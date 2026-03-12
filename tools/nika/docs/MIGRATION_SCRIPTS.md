# Nika v0.28 Workspace Migration Scripts

**Purpose:** Scripts to safely migrate from monolithic to 3-crate workspace.

---

## Script 1: Version Synchronization Check

**File:** `scripts/verify-versions.sh`

```bash
#!/bin/bash
# verify-versions.sh
# Verifies all crates have synchronized versions

set -euo pipefail

echo "=== Nika Workspace Version Check ==="

# Extract versions using cargo metadata
CORE_VER=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "nika-core") | .version')
RUNTIME_VER=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "nika-runtime") | .version')
TUI_VER=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "nika-tui") | .version')

echo "nika-core:    $CORE_VER"
echo "nika-runtime: $RUNTIME_VER"
echo "nika-tui:     $TUI_VER"

if [ "$CORE_VER" != "$RUNTIME_VER" ] || [ "$RUNTIME_VER" != "$TUI_VER" ]; then
  echo ""
  echo "❌ ERROR: Version mismatch detected"
  echo ""
  echo "All crates must have the same version for workspace releases."
  echo "Use: cargo set-version --workspace <version>"
  exit 1
fi

echo ""
echo "✅ All crates synchronized at v$CORE_VER"
exit 0
```

**Usage:**
```bash
chmod +x scripts/verify-versions.sh
./scripts/verify-versions.sh
```

---

## Script 2: Workspace Version Bump

**File:** `scripts/bump-version.sh`

```bash
#!/bin/bash
# bump-version.sh <major|minor|patch>
# Bumps version for all crates in workspace

set -euo pipefail

BUMP_TYPE="${1:-patch}"

if [[ ! "$BUMP_TYPE" =~ ^(major|minor|patch)$ ]]; then
  echo "Usage: $0 <major|minor|patch>"
  exit 1
fi

echo "=== Bumping workspace version ($BUMP_TYPE) ==="

# Require cargo-edit
if ! command -v cargo-set-version &> /dev/null; then
  echo "Installing cargo-edit..."
  cargo install cargo-edit
fi

# Get current version
CURRENT_VER=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "nika-core") | .version')
echo "Current version: v$CURRENT_VER"

# Bump version
cargo set-version --workspace --bump "$BUMP_TYPE"

# Get new version
NEW_VER=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "nika-core") | .version')
echo "New version: v$NEW_VER"

# Verify all crates updated
./scripts/verify-versions.sh

echo ""
echo "✅ Version bump complete: v$CURRENT_VER → v$NEW_VER"
echo ""
echo "Next steps:"
echo "  1. Update CHANGELOG.md with v$NEW_VER section"
echo "  2. Commit changes: git commit -am \"chore: bump version to v$NEW_VER\""
echo "  3. Tag release: git tag v$NEW_VER"
echo "  4. Push: git push && git push --tags"
```

**Usage:**
```bash
chmod +x scripts/bump-version.sh
./scripts/bump-version.sh patch   # 0.28.0 → 0.28.1
./scripts/bump-version.sh minor   # 0.28.0 → 0.29.0
./scripts/bump-version.sh major   # 0.28.0 → 1.0.0 (never for Nika!)
```

---

## Script 3: Pre-Publish Validation

**File:** `scripts/pre-publish-check.sh`

```bash
#!/bin/bash
# pre-publish-check.sh <crate-name>
# Validates a crate is ready for publishing to crates.io

set -euo pipefail

CRATE_NAME="${1:-}"

if [ -z "$CRATE_NAME" ]; then
  echo "Usage: $0 <crate-name>"
  echo "Example: $0 nika-core"
  exit 1
fi

echo "=== Pre-Publish Check: $CRATE_NAME ==="

# 1. Dry-run publish
echo ""
echo "1/6 Testing cargo publish (dry-run)..."
cargo publish -p "$CRATE_NAME" --dry-run --allow-dirty

# 2. Check documentation builds
echo ""
echo "2/6 Building documentation..."
cargo doc -p "$CRATE_NAME" --no-deps

# 3. Verify dependency tree
echo ""
echo "3/6 Checking dependency tree..."
cargo tree -p "$CRATE_NAME" --depth 1

# 4. Check README.md exists
echo ""
echo "4/6 Verifying README.md..."
CRATE_DIR=$(cargo metadata --format-version 1 --no-deps | jq -r ".packages[] | select(.name == \"$CRATE_NAME\") | .manifest_path" | xargs dirname)
if [ ! -f "$CRATE_DIR/README.md" ]; then
  echo "❌ ERROR: $CRATE_DIR/README.md not found"
  exit 1
fi

# 5. Verify license
echo ""
echo "5/6 Verifying LICENSE..."
if [ ! -f "LICENSE" ]; then
  echo "❌ ERROR: LICENSE file not found"
  exit 1
fi

# 6. Check version in CHANGELOG
echo ""
echo "6/6 Verifying CHANGELOG entry..."
VERSION=$(cargo metadata --format-version 1 --no-deps | jq -r ".packages[] | select(.name == \"$CRATE_NAME\") | .version")
if ! grep -q "## \[$VERSION\]" CHANGELOG.md; then
  echo "⚠️  WARNING: No CHANGELOG entry for v$VERSION"
fi

echo ""
echo "✅ All checks passed for $CRATE_NAME"
```

**Usage:**
```bash
chmod +x scripts/pre-publish-check.sh
./scripts/pre-publish-check.sh nika-core
./scripts/pre-publish-check.sh nika-runtime
./scripts/pre-publish-check.sh nika-tui
```

---

## Script 4: Publish Workflow

**File:** `scripts/publish-crates.sh`

```bash
#!/bin/bash
# publish-crates.sh
# Publishes all crates in dependency order with propagation delays

set -euo pipefail

# Check for token
if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  echo "❌ ERROR: CARGO_REGISTRY_TOKEN environment variable not set"
  echo ""
  echo "Get your token from: https://crates.io/settings/tokens"
  echo "Then: export CARGO_REGISTRY_TOKEN=<your-token>"
  exit 1
fi

echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
echo "║                                                                               ║"
echo "║   🦋  N I K A   W O R K S P A C E   P U B L I S H                            ║"
echo "║                                                                               ║"
echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
echo ""

# Verify versions are synchronized
echo "Step 0/7: Verifying version synchronization..."
./scripts/verify-versions.sh || exit 1

VERSION=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "nika-core") | .version')
echo ""
echo "Publishing version: v$VERSION"
echo ""
read -p "Continue with publishing? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 1
fi

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 1: Publish nika-core
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 1/7: Publishing nika-core..."
./scripts/pre-publish-check.sh nika-core || exit 1
cargo publish -p nika-core --token "$CARGO_REGISTRY_TOKEN"

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 2: Wait for propagation
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 2/7: Waiting 60s for crates.io index propagation..."
sleep 60

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 3: Verify nika-core availability
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 3/7: Verifying nika-core availability..."
if cargo search nika-core --limit 1 | grep -q "$VERSION"; then
  echo "✅ nika-core v$VERSION is available on crates.io"
else
  echo "❌ ERROR: nika-core v$VERSION not found on crates.io"
  exit 1
fi

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 4: Publish nika-runtime
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 4/7: Publishing nika-runtime..."
./scripts/pre-publish-check.sh nika-runtime || exit 1
cargo publish -p nika-runtime --token "$CARGO_REGISTRY_TOKEN"

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 5: Wait for propagation
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 5/7: Waiting 60s for crates.io index propagation..."
sleep 60

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 6: Verify nika-runtime availability
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 6/7: Verifying nika-runtime availability..."
if cargo search nika-runtime --limit 1 | grep -q "$VERSION"; then
  echo "✅ nika-runtime v$VERSION is available on crates.io"
else
  echo "❌ ERROR: nika-runtime v$VERSION not found on crates.io"
  exit 1
fi

# ═══════════════════════════════════════════════════════════════════════════════════
# Step 7: Publish nika-tui
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Step 7/7: Publishing nika-tui..."
./scripts/pre-publish-check.sh nika-tui || exit 1
cargo publish -p nika-tui --token "$CARGO_REGISTRY_TOKEN"

# ═══════════════════════════════════════════════════════════════════════════════════
# Final verification
# ═══════════════════════════════════════════════════════════════════════════════════

echo ""
echo "Final verification: Waiting 30s for final propagation..."
sleep 30

if cargo search nika-tui --limit 1 | grep -q "$VERSION"; then
  echo "✅ nika-tui v$VERSION is available on crates.io"
else
  echo "⚠️  WARNING: nika-tui v$VERSION not yet visible on crates.io (may take a few more minutes)"
fi

echo ""
echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
echo "║                                                                               ║"
echo "║   ✅  P U B L I S H   C O M P L E T E                                         ║"
echo "║                                                                               ║"
echo "║   Version: v$VERSION                                                          ║"
echo "║                                                                               ║"
echo "║   Published crates:                                                           ║"
echo "║   • nika-core                                                                 ║"
echo "║   • nika-runtime                                                              ║"
echo "║   • nika-tui                                                                  ║"
echo "║                                                                               ║"
echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Next steps:"
echo "  1. Verify on crates.io:"
echo "     - https://crates.io/crates/nika-core"
echo "     - https://crates.io/crates/nika-runtime"
echo "     - https://crates.io/crates/nika-tui"
echo "  2. Create GitHub release: gh release create v$VERSION"
echo "  3. Update Homebrew formula"
echo "  4. Announce release"
```

**Usage:**
```bash
chmod +x scripts/publish-crates.sh
export CARGO_REGISTRY_TOKEN=<your-token>
./scripts/publish-crates.sh
```

---

## Script 5: Rollback Failed Publish

**File:** `scripts/rollback-publish.sh`

```bash
#!/bin/bash
# rollback-publish.sh <crate-name> <version>
# Yanks a published crate version from crates.io

set -euo pipefail

CRATE_NAME="${1:-}"
VERSION="${2:-}"

if [ -z "$CRATE_NAME" ] || [ -z "$VERSION" ]; then
  echo "Usage: $0 <crate-name> <version>"
  echo "Example: $0 nika-runtime 0.28.0"
  exit 1
fi

# Check for token
if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  echo "❌ ERROR: CARGO_REGISTRY_TOKEN environment variable not set"
  exit 1
fi

echo "=== Rollback: Yanking $CRATE_NAME v$VERSION ==="
echo ""
echo "⚠️  WARNING: This will yank the crate from crates.io"
echo "   It will no longer be available for installation"
echo ""
read -p "Continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 1
fi

cargo yank --vers "$VERSION" "$CRATE_NAME" --token "$CARGO_REGISTRY_TOKEN"

echo ""
echo "✅ $CRATE_NAME v$VERSION has been yanked"
echo ""
echo "Next steps:"
echo "  1. Fix the issue in code"
echo "  2. Bump PATCH version: ./scripts/bump-version.sh patch"
echo "  3. Retry publishing: ./scripts/publish-crates.sh"
```

**Usage:**
```bash
chmod +x scripts/rollback-publish.sh
export CARGO_REGISTRY_TOKEN=<your-token>
./scripts/rollback-publish.sh nika-runtime 0.28.0
```

---

## Script 6: CI Dry-Run Test

**File:** `scripts/test-ci-locally.sh`

```bash
#!/bin/bash
# test-ci-locally.sh
# Simulates CI checks locally before pushing

set -euo pipefail

echo "=== Local CI Simulation ==="

FAILED=0

# Phase 1: Format
echo ""
echo "Phase 1/7: Format check..."
if cargo fmt --all --check; then
  echo "✅ Format check passed"
else
  echo "❌ Format check failed"
  FAILED=1
fi

# Phase 2: Clippy (all crates)
echo ""
echo "Phase 2/7: Clippy check..."
for crate in nika-core nika-runtime nika-tui; do
  echo "  Checking $crate..."
  if cargo clippy -p "$crate" --all-targets -- -D warnings; then
    echo "  ✅ $crate clippy passed"
  else
    echo "  ❌ $crate clippy failed"
    FAILED=1
  fi
done

# Phase 3: Core tests
echo ""
echo "Phase 3/7: nika-core tests..."
if cargo nextest run -p nika-core; then
  echo "✅ nika-core tests passed"
else
  echo "❌ nika-core tests failed"
  FAILED=1
fi

# Phase 4: Runtime tests
echo ""
echo "Phase 4/7: nika-runtime tests..."
if cargo nextest run -p nika-runtime; then
  echo "✅ nika-runtime tests passed"
else
  echo "❌ nika-runtime tests failed"
  FAILED=1
fi

# Phase 5: TUI tests
echo ""
echo "Phase 5/7: nika-tui tests..."
if cargo nextest run -p nika-tui; then
  echo "✅ nika-tui tests passed"
else
  echo "❌ nika-tui tests failed"
  FAILED=1
fi

# Phase 6: Documentation
echo ""
echo "Phase 6/7: Documentation build..."
if cargo doc --workspace --no-deps --all-features; then
  echo "✅ Documentation build passed"
else
  echo "❌ Documentation build failed"
  FAILED=1
fi

# Phase 7: Security
echo ""
echo "Phase 7/7: Security checks..."
if command -v cargo-deny &> /dev/null; then
  if cargo deny check; then
    echo "✅ cargo-deny passed"
  else
    echo "⚠️  cargo-deny warnings (see output above)"
  fi
else
  echo "⚠️  cargo-deny not installed (skipping)"
fi

echo ""
echo "========================================"
if [ $FAILED -eq 0 ]; then
  echo "✅ All local CI checks passed"
  echo ""
  echo "Ready to push to GitHub!"
  exit 0
else
  echo "❌ Some checks failed"
  echo ""
  echo "Fix the issues above before pushing."
  exit 1
fi
```

**Usage:**
```bash
chmod +x scripts/test-ci-locally.sh
./scripts/test-ci-locally.sh
```

---

## Installation Instructions

1. **Create scripts directory:**
   ```bash
   mkdir -p scripts
   ```

2. **Copy all scripts:**
   ```bash
   # Copy each script from this document to scripts/
   # Make them executable
   chmod +x scripts/*.sh
   ```

3. **Install dependencies:**
   ```bash
   # cargo-edit for version bumping
   cargo install cargo-edit

   # cargo-nextest for faster tests
   cargo install cargo-nextest

   # cargo-deny for security checks
   cargo install cargo-deny

   # jq for JSON parsing
   brew install jq  # macOS
   sudo apt-get install jq  # Ubuntu
   ```

4. **Test scripts:**
   ```bash
   ./scripts/verify-versions.sh
   ./scripts/test-ci-locally.sh
   ```

---

## Workflow: Complete Release Process

```bash
# 1. Ensure clean working tree
git status

# 2. Bump version
./scripts/bump-version.sh patch  # 0.28.0 → 0.28.1

# 3. Update CHANGELOG.md
vim CHANGELOG.md  # Add v0.28.1 section

# 4. Run local CI
./scripts/test-ci-locally.sh

# 5. Commit changes
git add .
git commit -m "chore: bump version to v0.28.1"

# 6. Tag release
git tag v0.28.1

# 7. Push (triggers CI)
git push && git push --tags

# 8. Wait for CI to pass
# Monitor: https://github.com/supernovae-st/nika/actions

# 9. Publish to crates.io
export CARGO_REGISTRY_TOKEN=<your-token>
./scripts/publish-crates.sh

# 10. Create GitHub release
gh release create v0.28.1 --generate-notes

# 11. Update Homebrew (automatic via workflow)
```

---

## Summary

These scripts provide:

1. **Version Management** — Synchronized bumping across all crates
2. **Pre-Publish Validation** — Catch issues before publishing
3. **Sequential Publishing** — Safe crate.io publishing with delays
4. **Rollback Support** — Yank failed releases
5. **Local CI** — Test before pushing to GitHub
6. **Automated Workflow** — Complete end-to-end release process

All scripts are production-ready and follow bash best practices.
