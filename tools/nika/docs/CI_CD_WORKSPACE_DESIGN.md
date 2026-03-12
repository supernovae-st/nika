# Nika v0.28 CI/CD Workspace Design

**Version:** v0.28
**Date:** 2026-03-12
**Status:** Design Complete

---

## Executive Summary

This document designs the complete CI/CD pipeline for Nika's v0.28 3-crate workspace transition from the current monolithic structure.

**Current State:** Single crate with monolithic CI
**Target State:** 3-crate workspace with modular CI/CD

---

## Table of Contents

1. [Workspace Structure](#workspace-structure)
2. [CI Pipeline Design](#ci-pipeline-design)
3. [Release Pipeline Design](#release-pipeline-design)
4. [Feature Flag Matrix](#feature-flag-matrix)
5. [Cross-Platform Testing](#cross-platform-testing)
6. [Crate Publishing Strategy](#crate-publishing-strategy)
7. [Verification Stages](#verification-stages)
8. [Migration Plan](#migration-plan)

---

## 1. Workspace Structure

### Target Layout

```
nika/
├── Cargo.toml                    # Workspace root
├── .github/
│   └── workflows/
│       ├── ci.yml               # Main CI workflow (NEW)
│       ├── release.yml          # Release workflow (UPDATED)
│       ├── comprehensive-tests.yml # Extended tests (UPDATED)
│       └── crate-publish.yml    # Crate publishing (NEW)
├── nika-core/                   # Crate 1: Core types
│   ├── Cargo.toml
│   ├── src/
│   └── tests/
├── nika-runtime/                # Crate 2: Execution engine
│   ├── Cargo.toml
│   ├── src/
│   └── tests/
└── nika-tui/                    # Crate 3: Terminal interface
    ├── Cargo.toml
    ├── src/
    └── tests/
```

### Dependency Graph

```
nika-tui (bin)
    ↓ depends on
nika-runtime (lib)
    ↓ depends on
nika-core (lib)
    ↓ no external deps (zero-dep core)
```

---

## 2. CI Pipeline Design

### Overview

The CI pipeline uses a **matrix strategy** to test each crate independently while respecting dependency order.

### Job Structure

```yaml
jobs:
  # Phase 1: Quick checks (parallel)
  format-check
  clippy-check

  # Phase 2: Core crate tests (foundation)
  test-core

  # Phase 3: Runtime crate tests (depends on core)
  test-runtime

  # Phase 4: TUI crate tests (depends on runtime)
  test-tui

  # Phase 5: Integration tests (all crates)
  integration-tests

  # Phase 6: Coverage & Security
  coverage
  security

  # Phase 7: Build verification
  build-verification

  # Phase 8: Summary
  summary
```

### Matrix Dimensions

#### Platform Matrix

```yaml
os: [ubuntu-latest, macos-latest, windows-latest]
rust: [stable, beta]
```

#### Feature Matrix (per crate)

**nika-core:**
- No features (zero-dep)

**nika-runtime:**
- `default` (base runtime)
- `spn-daemon` (unified secret management)
- `native-keychain` (OS keychain)
- `native-inference` (mistral.rs)
- `all` (all features)

**nika-tui:**
- `default` (full TUI)
- `lsp` (Language Server Protocol)
- `jobs` (Jobs Daemon)
- `docker` (Docker build profile)
- `all` (all features)

### Caching Strategy

```yaml
cache:
  key: |
    ${{ runner.os }}-
    ${{ matrix.rust }}-
    ${{ hashFiles('**/Cargo.lock') }}-
    ${{ matrix.crate }}
  paths:
    - ~/.cargo/registry
    - ~/.cargo/git
    - target/${{ matrix.crate }}
```

---

## 3. Release Pipeline Design

### Release Workflow Stages

```
1. Version Validation
   ├─ Check version coherence (core → runtime → tui)
   └─ Verify CHANGELOG entries exist

2. Build Matrix (6 targets)
   ├─ macOS ARM64 (native features)
   ├─ macOS x64 (native features)
   ├─ Linux ARM64 (native features)
   ├─ Linux x64 (native features)
   ├─ Linux musl x64 (docker features)
   └─ Linux musl ARM64 (docker features)

3. Test Suite (all platforms)
   ├─ Unit tests per crate
   ├─ Integration tests
   └─ Feature flag combinations

4. Publish Crates (sequential)
   ├─ 1. nika-core
   ├─ 2. Wait for crates.io propagation (60s)
   ├─ 3. nika-runtime
   ├─ 4. Wait for propagation (60s)
   └─ 5. nika-tui

5. GitHub Release
   ├─ Create release with binaries
   ├─ Generate release notes
   └─ Attach checksums

6. Docker Publish
   ├─ Multi-arch image (amd64, arm64)
   └─ Tag: latest, v0.X.Y, v0.X, v0

7. Homebrew Update
   └─ Update formula with new version
```

### Version Synchronization

**Rule:** All crates MUST have synchronized PATCH versions.

```
nika-core:    0.28.0
nika-runtime: 0.28.0
nika-tui:     0.28.0
```

**Automated Check:**

```bash
#!/bin/bash
# verify-versions.sh

CORE_VER=$(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "nika-core") | .version')
RUNTIME_VER=$(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "nika-runtime") | .version')
TUI_VER=$(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "nika-tui") | .version')

if [ "$CORE_VER" != "$RUNTIME_VER" ] || [ "$RUNTIME_VER" != "$TUI_VER" ]; then
  echo "ERROR: Version mismatch detected"
  echo "  nika-core:    $CORE_VER"
  echo "  nika-runtime: $RUNTIME_VER"
  echo "  nika-tui:     $TUI_VER"
  exit 1
fi

echo "✓ All crates synchronized at v$CORE_VER"
```

---

## 4. Feature Flag Matrix

### Test Combinations

**Strategy:** Test critical feature combinations, not all permutations (2^N explosion).

#### nika-runtime Feature Matrix

```yaml
features:
  # Baseline
  - name: "no-default"
    flags: "--no-default-features"

  # Common combinations
  - name: "default"
    flags: ""

  - name: "daemon-only"
    flags: "--no-default-features --features spn-daemon"

  - name: "native-only"
    flags: "--no-default-features --features native-inference"

  - name: "keychain-only"
    flags: "--no-default-features --features native-keychain"

  # Full features
  - name: "all"
    flags: "--all-features"
```

#### nika-tui Feature Matrix

```yaml
features:
  # Baseline
  - name: "no-default"
    flags: "--no-default-features"

  # Common combinations
  - name: "default"
    flags: ""

  - name: "lsp-only"
    flags: "--no-default-features --features lsp"

  - name: "jobs-only"
    flags: "--no-default-features --features jobs"

  - name: "docker"
    flags: "--no-default-features --features docker"

  # Full features
  - name: "all"
    flags: "--all-features"
```

### Test Matrix Size

```
Platforms: 3 (ubuntu, macos, windows)
Rust Versions: 2 (stable, beta)
Crates: 3 (core, runtime, tui)
Feature Combos: 6 per crate (avg)

Total: 3 × 2 × 3 × 6 = 108 test jobs
```

**Optimization:** Run beta only on ubuntu for PRs (reduce to 60 jobs).

---

## 5. Cross-Platform Testing

### Platform-Specific Considerations

#### Ubuntu (Primary)

```yaml
- name: Install dependencies (Ubuntu)
  if: runner.os == 'Linux'
  run: |
    sudo apt-get update
    sudo apt-get install -y \
      libdbus-1-dev \
      pkg-config \
      musl-tools
```

#### macOS

```yaml
- name: Install dependencies (macOS)
  if: runner.os == 'macOS'
  run: |
    brew install pkg-config
    # No libdbus needed (macOS native APIs)
```

#### Windows

```yaml
- name: Install dependencies (Windows)
  if: runner.os == 'Windows'
  run: |
    # Windows native APIs, no extra deps
    choco install llvm  # For native-inference
```

### Platform Test Coverage

| Feature | Ubuntu | macOS | Windows |
|---------|--------|-------|---------|
| Core (zero-dep) | ✅ | ✅ | ✅ |
| Runtime (base) | ✅ | ✅ | ✅ |
| spn-daemon | ✅ | ✅ | ❌ (Unix only) |
| native-keychain | ✅ | ✅ | ✅ |
| native-inference | ✅ | ✅ | ⚠️ (CPU only) |
| TUI (default) | ✅ | ✅ | ⚠️ (limited) |
| LSP | ✅ | ✅ | ✅ |
| Jobs | ✅ | ✅ | ✅ |

---

## 6. Crate Publishing Strategy

### Publishing Order

**CRITICAL:** Must publish in dependency order with propagation delays.

```bash
# 1. Publish nika-core (no deps)
cargo publish -p nika-core --token $CRATES_IO_TOKEN

# 2. Wait for crates.io index propagation
sleep 60

# 3. Verify nika-core is available
cargo search nika-core --limit 1 | grep "0.28.0"

# 4. Publish nika-runtime (depends on nika-core)
cargo publish -p nika-runtime --token $CRATES_IO_TOKEN

# 5. Wait for propagation
sleep 60

# 6. Verify nika-runtime is available
cargo search nika-runtime --limit 1 | grep "0.28.0"

# 7. Publish nika-tui (depends on nika-runtime)
cargo publish -p nika-tui --token $CRATES_IO_TOKEN

# 8. Final verification
sleep 30
cargo search nika-tui --limit 1 | grep "0.28.0"
```

### Rollback Strategy

If publishing fails at any stage:

```bash
# Yank failed crate
cargo yank --vers 0.28.0 nika-runtime

# Fix issue in code
# ...

# Bump PATCH version (0.28.0 → 0.28.1)
cargo set-version --workspace --bump patch

# Retry publishing sequence
```

### Pre-Publish Checks

```yaml
- name: Pre-publish verification
  run: |
    # 1. Dry-run publish
    cargo publish -p nika-core --dry-run --allow-dirty

    # 2. Check documentation builds
    cargo doc -p nika-core --no-deps

    # 3. Verify no dev-dependencies leaked
    cargo tree -p nika-core --depth 1

    # 4. Check README.md exists
    test -f nika-core/README.md

    # 5. Verify license file
    test -f LICENSE
```

---

## 7. Verification Stages

### Stage 1: Format & Lint (2 min)

```yaml
format-check:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: rustfmt
    - run: cargo fmt --all --check

clippy-check:
  runs-on: ubuntu-latest
  strategy:
    matrix:
      crate: [nika-core, nika-runtime, nika-tui]
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy
    - run: cargo clippy -p ${{ matrix.crate }} --all-targets -- -D warnings
```

### Stage 2: Unit Tests (10 min per crate)

```yaml
test-core:
  needs: [format-check, clippy-check]
  strategy:
    matrix:
      os: [ubuntu-latest, macos-latest]
      rust: [stable]
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@${{ matrix.rust }}
    - uses: Swatinem/rust-cache@v2
    - run: cargo nextest run -p nika-core

test-runtime:
  needs: test-core
  strategy:
    matrix:
      os: [ubuntu-latest, macos-latest]
      rust: [stable]
      features:
        - ""
        - "--no-default-features"
        - "--features spn-daemon"
        - "--features native-inference"
        - "--all-features"
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@${{ matrix.rust }}
    - uses: Swatinem/rust-cache@v2
    - run: cargo nextest run -p nika-runtime ${{ matrix.features }}

test-tui:
  needs: test-runtime
  strategy:
    matrix:
      os: [ubuntu-latest, macos-latest]
      rust: [stable]
      features:
        - ""
        - "--features lsp"
        - "--features jobs"
        - "--features docker"
        - "--all-features"
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@${{ matrix.rust }}
    - uses: Swatinem/rust-cache@v2
    - run: cargo nextest run -p nika-tui ${{ matrix.features }}
```

### Stage 3: Integration Tests (15 min)

```yaml
integration-tests:
  needs: [test-core, test-runtime, test-tui]
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2

    # Test workspace as a whole
    - run: cargo test --workspace --test '*'

    # Test example workflows
    - run: cargo run --release -- check examples/v15-*.nika.yaml
```

### Stage 4: Coverage (20 min)

```yaml
coverage:
  needs: integration-tests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: llvm-tools-preview
    - uses: taiki-e/install-action@cargo-llvm-cov
    - uses: Swatinem/rust-cache@v2

    # Generate coverage per crate
    - run: |
        cargo llvm-cov --workspace --lcov --output-path lcov.info
        cargo llvm-cov report --workspace

    - uses: codecov/codecov-action@v5
      with:
        files: lcov.info
        flags: nika-workspace
      env:
        CODECOV_TOKEN: ${{ secrets.CODECOV_TOKEN }}
```

### Stage 5: Security Audit (5 min)

```yaml
security:
  needs: integration-tests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
    - uses: taiki-e/install-action@v2
      with:
        tool: cargo-deny,cargo-audit

    # Check each crate
    - run: cargo deny check -D warnings
    - run: cargo audit --deny warnings
```

### Stage 6: Documentation (5 min)

```yaml
docs:
  needs: integration-tests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2

    # Build docs for each crate
    - run: cargo doc --workspace --no-deps
      env:
        RUSTDOCFLAGS: -D warnings

    # Check README.md files exist
    - run: |
        test -f nika-core/README.md
        test -f nika-runtime/README.md
        test -f nika-tui/README.md
```

### Stage 7: MSRV Check (5 min)

```yaml
msrv:
  needs: integration-tests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@1.86  # Minimum version
    - uses: Swatinem/rust-cache@v2

    # Verify MSRV for each crate
    - run: cargo check -p nika-core
    - run: cargo check -p nika-runtime
    - run: cargo check -p nika-tui
```

---

## 8. Migration Plan

### Phase 1: Preparation (Week 1)

**Tasks:**
1. Create workspace root `Cargo.toml`
2. Create crate directories: `nika-core/`, `nika-runtime/`, `nika-tui/`
3. Split code:
   - Move `src/ast/`, `src/dag/`, `src/error.rs` → `nika-core/src/`
   - Move `src/runtime/`, `src/mcp/`, `src/provider/` → `nika-runtime/src/`
   - Move `src/tui/`, `src/main.rs` → `nika-tui/src/`
4. Update imports across crates
5. Verify local builds: `cargo build --workspace`

**Validation:**
- All tests pass locally
- Zero clippy warnings
- Documentation builds successfully

### Phase 2: CI Migration (Week 2)

**Tasks:**
1. Create new `.github/workflows/ci.yml` (from this design)
2. Update `.github/workflows/release.yml` with workspace support
3. Add `.github/workflows/crate-publish.yml` workflow
4. Test CI on feature branch
5. Fix any CI failures

**Validation:**
- All CI jobs pass on test branch
- Coverage maintained at 70%+
- Build time under 30 minutes

### Phase 3: Release Automation (Week 3)

**Tasks:**
1. Add version synchronization script
2. Create release checklist
3. Add automated changelog generation
4. Configure crates.io publishing
5. Test dry-run release

**Validation:**
- `cargo publish --dry-run` succeeds for all crates
- Release notes generate correctly
- Version bumping script works

### Phase 4: Deployment (Week 4)

**Tasks:**
1. Merge workspace PR to main
2. Tag v0.28.0 release
3. Monitor CI/CD pipeline
4. Publish to crates.io
5. Update documentation

**Validation:**
- All crates published successfully
- GitHub release created
- Docker image available
- Homebrew formula updated

---

## Appendix A: Complete CI Workflow

See: `CI_WORKFLOW_COMPLETE.yml` (next file to generate)

---

## Appendix B: Complete Release Workflow

See: `RELEASE_WORKFLOW_COMPLETE.yml` (next file to generate)

---

## Appendix C: Crate Publishing Workflow

See: `CRATE_PUBLISH_WORKFLOW.yml` (next file to generate)

---

## Summary

This design provides:

1. **Modular CI**: Independent per-crate testing with dependency ordering
2. **Feature Matrix**: Comprehensive feature flag testing (108 jobs optimized to 60)
3. **Cross-Platform**: Ubuntu, macOS, Windows support
4. **Sequential Publishing**: Safe crate publishing with propagation delays
5. **Verification Gates**: 7 stages (format, test, integration, coverage, security, docs, MSRV)
6. **Automated Release**: One-command release with version synchronization

**Total CI Time:** ~35 minutes for full pipeline
**Total Release Time:** ~60 minutes including crate propagation

---

**Next Steps:**
1. Generate complete workflow YAML files
2. Create migration scripts
3. Test on feature branch
4. Deploy to main
