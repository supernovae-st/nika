# Handoff C: Distribution Simplification

> **Copy-paste this into a new Claude Code session to execute.**
> Estimated time: 2-3 days | Tests baseline: 10,315
> **Prereq**: Handoff A (LSP decoupling) for Phase 3 (VSIX bundling).
> Phases 1-2 can start NOW.

## Mission

Fix broken distribution channels. All 9 channels at same version.
1 download = everything. No feature flag decisions for users.

```
BEFORE: npm at v0.46.1 (27 behind), VS Code at v0.51.0 (22 behind), no binary in VSIX
AFTER:  All channels at v0.74.0+, binary bundled in extension, version sync automated
```

## Context

- Cargo workspace: v0.73.0 (v0.74.0 changelog written, ready to tag)
- npm: v0.46.1 (BROKEN — 27 versions behind)
- VS Code extension: v0.51.0 (22 versions behind)
- Homebrew: v0.72.0 (1 behind, auto-fixes on tag)
- native-inference NOW bundled by default → users get EVERYTHING
- 7 build targets, 9 distribution channels, all CI automated
- Release pipeline: `.github/workflows/release.yml` (1543 lines)

## Pre-Flight

```bash
cd /Users/thibaut/dev/supernovae/nika
git status  # Should be clean
grep '^version' tools/Cargo.toml  # 0.73.0
node -p "require('./packages/npm/package.json').version"  # 0.46.1 (BROKEN)
node -p "require('./editors/vscode/package.json').version"  # 0.51.0 (BEHIND)
```

## Mandatory Skills

- `verification-before-completion`
- `shell-scripting:bash-defensive-patterns` (for sync scripts)

## Phase 1: Fix npm Version Desync (2-3 hours — START HERE)

### Task 1.1: Update All npm Package Versions

Update these files to match Cargo version (currently 0.73.0, will be 0.74.0 at tag time):

```
packages/npm/package.json                    0.46.1 -> 0.74.0
packages/nika-darwin-arm64/package.json      0.46.1 -> 0.74.0
packages/nika-darwin-x64/package.json        0.46.1 -> 0.74.0
packages/nika-linux-x64/package.json         0.46.1 -> 0.74.0
packages/nika-linux-arm64/package.json       0.46.1 -> 0.74.0
packages/nika-win32-x64/package.json         0.46.1 -> 0.74.0
```

Also update optionalDependencies versions in main package.

**IMPORTANT**: Use the version that will be tagged next (check tools/Cargo.toml).
If v0.74.0 is ready, use 0.74.0.

```
Commit: chore(npm): sync npm package versions to v0.74.0

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Task 1.2: Verify npm Publish Job in release.yml

Check `.github/workflows/release.yml` the `npm-publish` job:
1. Does it download the correct binary artifact for each platform?
2. Does it publish platform packages BEFORE the main wrapper?
3. Does it use `NPM_TOKEN` secret?

If broken, fix the job.

### Task 1.3: Add Version Sync Guard

Add a preflight check to release.yml that fails if versions don't match:

```yaml
- name: Verify version coherence
  run: |
    CARGO_VERSION=$(grep '^version' tools/Cargo.toml | head -1 | cut -d'"' -f2)
    NPM_VERSION=$(node -p "require('./packages/npm/package.json').version")
    VSCODE_VERSION=$(node -p "require('./editors/vscode/package.json').version")
    if [ "$CARGO_VERSION" != "$NPM_VERSION" ]; then
      echo "::error::npm version $NPM_VERSION != Cargo version $CARGO_VERSION"
      exit 1
    fi
    if [ "$CARGO_VERSION" != "$VSCODE_VERSION" ]; then
      echo "::error::vscode version $VSCODE_VERSION != Cargo version $CARGO_VERSION"
      exit 1
    fi
```

```
Commit: feat(ci): add cross-channel version sync guard

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Phase 2: VS Code Version + Sync Script (1-2 hours)

### Task 2.1: Update Extension Version

```
editors/vscode/package.json  0.51.0 -> 0.74.0
```

Keep `engines.vscode: "^1.75.0"` unchanged.

```
Commit: chore(vscode): sync extension version to v0.74.0

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Task 2.2: Create Version Sync Script

Create `scripts/sync-versions.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

VERSION=$(grep '^version' tools/Cargo.toml | head -1 | cut -d'"' -f2)
echo "Syncing all packages to v${VERSION}"

# npm packages
for pkg in packages/*/package.json; do
  if command -v jq >/dev/null 2>&1; then
    jq --arg v "$VERSION" '.version = $v' "$pkg" > "${pkg}.tmp" && mv "${pkg}.tmp" "$pkg"
  else
    sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"${VERSION}\"/" "$pkg"
  fi
done

# npm optional dependencies
if command -v jq >/dev/null 2>&1; then
  jq --arg v "$VERSION" '
    .optionalDependencies |= with_entries(.value = $v)
  ' packages/npm/package.json > packages/npm/package.json.tmp \
    && mv packages/npm/package.json.tmp packages/npm/package.json
fi

# VS Code extension
if command -v jq >/dev/null 2>&1; then
  jq --arg v "$VERSION" '.version = $v' editors/vscode/package.json \
    > editors/vscode/package.json.tmp \
    && mv editors/vscode/package.json.tmp editors/vscode/package.json
else
  sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"${VERSION}\"/" editors/vscode/package.json
fi

echo "All packages synced to v${VERSION}"
echo "  Cargo:  ${VERSION}"
echo "  npm:    $(node -p "require('./packages/npm/package.json').version")"
echo "  vscode: $(node -p "require('./editors/vscode/package.json').version")"
```

```bash
chmod +x scripts/sync-versions.sh
```

```
Commit: feat(scripts): add version sync script for cross-channel alignment

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Phase 3: Platform VSIX Bundling (1 day — needs Handoff A done)

### Task 3.1: Add .vscodeignore

Create `editors/vscode/.vscodeignore`:
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

### Task 3.2: Add findBundledBinary()

In `editors/vscode/src/extension.ts`, add binary discovery:
```typescript
import * as fs from 'fs';
import * as path from 'path';

function findBundledBinary(context: ExtensionContext): string | null {
  const bin = process.platform === 'win32' ? 'nika.exe' : 'nika';
  const bundled = path.join(context.extensionPath, 'server', bin);
  return fs.existsSync(bundled) ? bundled : null;
}
```

Update binary discovery priority in `activate()`:
1. `nika.server.path` config setting
2. `findBundledBinary(context)` — NEW
3. PATH lookup
4. Cached download
5. Auto-download from GitHub

### Task 3.3: Update CI for Platform Matrix

Replace `vscode-publish` job with matrix build (6 platforms + universal).
See `docs/plans/2026-04-06-plan-3-distribution-single-download.md` Phase 3 for full YAML.

```
Commit: feat(ci): platform-specific VSIX with bundled nika binary

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Test locally:
```bash
cd tools && cargo build --release -p nika
mkdir -p ../editors/vscode/server
cp ../target/release/nika ../editors/vscode/server/
cd ../editors/vscode
npx vsce package --no-git-tag-version --target darwin-arm64
ls -lh *.vsix  # Should be ~18MB (not 32KB)
```

## Phase 4: Simplified Install Docs (1 hour)

### Task 4.1: Update README Install Section

4 methods, no feature flags:

```markdown
## Install

### macOS
brew install supernovae-st/tap/nika

### Any platform (npm)
npm install -g @supernovae-st/nika

### VS Code / Cursor
Search "Nika" in extensions — Install (binary included!)

### Direct download
https://github.com/supernovae-st/nika/releases/latest
```

**Local models** (separate section):
```markdown
## Local Models (Advanced)
cargo install nika --features native-inference
```

Note: native-inference is now bundled by default in release binaries,
so this section may be unnecessary. Verify by checking feature flags
in `tools/nika/Cargo.toml`.

## Phase 5: Bump Script + Smoke Test (1 hour)

### Task 5.1: Create scripts/bump-version.sh

```bash
#!/usr/bin/env bash
set -euo pipefail
NEW_VERSION="${1:?Usage: bump-version.sh <version>}"
echo "Bumping to v${NEW_VERSION}"

# Cargo workspace
sed -i '' "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" tools/Cargo.toml

# Sync all others
./scripts/sync-versions.sh

echo "Done. Now: git add -A && git commit -m 'chore: bump version to v${NEW_VERSION}'"
```

### Task 5.2: Add Smoke Test to release.yml

Post-publish verification that all channels have the correct version.

## Verification Checklist

```bash
# Version alignment
grep '^version' tools/Cargo.toml
node -p "require('./packages/npm/package.json').version"
node -p "require('./editors/vscode/package.json').version"
# ALL must match

# npm dry-run
cd packages/npm && npm publish --dry-run

# VSIX builds (after Phase 3)
cd editors/vscode && npx vsce package --no-git-tag-version --target darwin-arm64
ls -lh *.vsix  # ~18MB

# Sync script idempotent
./scripts/sync-versions.sh && git diff  # Should show NO changes

# All tests still pass
cd tools && cargo test --workspace --lib
```

## Gotchas

1. **npm publish order**: Platform packages BEFORE main wrapper
2. **VSIX 200MB limit**: Our ~18MB is fine
3. **Homebrew SHA256 race**: Assets must be fully uploaded before formula update
4. **npm token scope**: Must have publish access to @supernovae-st
5. **crates.io order**: 13 crates in dependency order, sleep 15s between each
6. **Windows unsigned**: SmartScreen warnings until SignPath configured

## Commit Convention

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**NEVER Claude/Anthropic co-author. ALWAYS Nika.**
