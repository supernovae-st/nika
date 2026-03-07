# Fix Nika Release Pipeline

**Date:** 2026-03-07
**Author:** Claude + Thibaut
**Status:** In Progress

## Executive Summary

The Nika release pipeline is broken due to:
1. Invalid GitHub Action versions (v6/v7/v8 don't exist)
2. Docker build failing due to cross-compilation issues (missing `aarch64-linux-musl-gcc`)
3. No artifacts attached to GitHub releases

This plan aligns Nika's release infrastructure with the proven spn-cli pattern.

## Current State Analysis

### Release Workflow Issues

```
╔═══════════════════════════════════════════════════════════════════════════════════╗
║  RELEASE.YML ISSUES                                                               ║
╠═══════════════════════════════════════════════════════════════════════════════════╣
║                                                                                   ║
║  1. Invalid action versions:                                                      ║
║     - actions/checkout@v6          → Should be @v4                               ║
║     - actions/upload-artifact@v7   → Should be @v4                               ║
║     - actions/download-artifact@v8 → Should be @v4                               ║
║                                                                                   ║
║  2. Build failure: v0.21.1 failed in 8 seconds (version job)                     ║
║     - Immediate failure due to invalid checkout@v6                               ║
║                                                                                   ║
║  3. Publish job references non-existent crates:                                  ║
║     - nika-core, nika-mcp, etc. (Nika is monolithic, no workspace crates)        ║
║                                                                                   ║
╚═══════════════════════════════════════════════════════════════════════════════════╝
```

### Docker Workflow Issues

```
╔═══════════════════════════════════════════════════════════════════════════════════╗
║  DOCKER.YML ISSUES                                                                ║
╠═══════════════════════════════════════════════════════════════════════════════════╣
║                                                                                   ║
║  1. Builds inside container (Alpine Rust image)                                  ║
║     - Cross-compilation for ARM64 fails                                          ║
║     - Missing aarch64-linux-musl-gcc in Alpine                                   ║
║     - aws-lc-sys requires native C compiler                                      ║
║                                                                                   ║
║  2. Error: "failed to find tool aarch64-linux-musl-gcc"                          ║
║     - gcc-aarch64-none-elf doesn't provide musl GCC                              ║
║                                                                                   ║
║  3. Uses dtolnay/rust-action@stable (typo: should be rust-toolchain)             ║
║                                                                                   ║
╚═══════════════════════════════════════════════════════════════════════════════════╝
```

## Solution: Align with spn-cli Pattern

### Architecture Comparison

```
CURRENT (Broken)                    TARGET (spn-cli pattern)
────────────────────────────────    ────────────────────────────────────
release.yml:                        release.yml:
├── version job                     ├── build job (6 targets)
├── build job (native only)         │   ├── aarch64-apple-darwin
├── release job                     │   ├── x86_64-apple-darwin
└── publish job (broken)            │   ├── aarch64-unknown-linux-gnu
                                    │   ├── x86_64-unknown-linux-gnu
docker.yml:                         │   ├── x86_64-unknown-linux-musl ◄─ Docker
├── build-and-push (builds inside)  │   └── aarch64-unknown-linux-musl ◄─ Docker
├── integration-tests               ├── docker-publish job
└── verify-image                    │   └── Uses pre-built musl binaries
                                    ├── release job
                                    │   └── Attaches all artifacts
                                    └── update-homebrew job
```

## Implementation Plan

### Phase 1: Fix Action Versions (Immediate)

**File:** `.github/workflows/release.yml`

```yaml
# BEFORE
- uses: actions/checkout@v6
- uses: actions/upload-artifact@v7
- uses: actions/download-artifact@v8

# AFTER
- uses: actions/checkout@v4
- uses: actions/upload-artifact@v4
- uses: actions/download-artifact@v4
```

### Phase 2: Add Musl Build Targets

**File:** `.github/workflows/release.yml`

Add to build matrix:
```yaml
matrix:
  include:
    # ... existing targets ...
    # === Docker targets (musl static, no keychain) ===
    - target: x86_64-unknown-linux-musl
      os: ubuntu-latest
      docker: true
    - target: aarch64-unknown-linux-musl
      os: ubuntu-latest
      docker: true
      cross: true
```

### Phase 3: Refactor Docker Build

**File:** `.github/workflows/release.yml`

Add docker-publish job (like spn-cli):
```yaml
docker-publish:
  name: Publish Docker Image
  needs: build
  runs-on: ubuntu-latest
  permissions:
    packages: write
    attestations: write
    id-token: write
  steps:
    - name: Download musl artifacts
      uses: actions/download-artifact@v4
      with:
        pattern: nika-*-musl-*
        path: artifacts
    # ... extract and build scratch image ...
```

### Phase 4: Simplify Dockerfile

**File:** `tools/nika/Dockerfile`

```dockerfile
# BEFORE: Multi-stage build with Rust compilation
FROM rust:1.94-alpine AS builder
# ... complex build logic ...

# AFTER: Simple scratch image with pre-built binary
FROM scratch
ARG TARGETARCH
COPY ${TARGETARCH}/nika /nika
WORKDIR /workspace
ENTRYPOINT ["/nika"]
CMD ["--help"]
```

### Phase 5: Remove Separate docker.yml

The docker build will be integrated into release.yml, triggered on tags only.
Keep docker.yml for PR builds only (build without push).

### Phase 6: Add Homebrew Formula Update

**File:** `.github/workflows/release.yml`

```yaml
update-homebrew:
  name: Update Homebrew Formula
  needs: release
  runs-on: ubuntu-latest
  steps:
    - uses: mislav/bump-homebrew-formula-action@v3
      with:
        formula-name: nika
        homebrew-tap: supernovae-st/homebrew-tap
        download-url: https://github.com/supernovae-st/nika/releases/download/${{ env.TAG }}/nika-x86_64-apple-darwin.tar.gz
      env:
        COMMITTER_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
```

## File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `.github/workflows/release.yml` | **REWRITE** | Fix versions, add musl targets, add docker job, add homebrew |
| `tools/nika/Dockerfile` | **SIMPLIFY** | Remove build logic, use pre-built binaries |
| `.github/workflows/docker.yml` | **MODIFY** | Keep for PR builds only, no push |

## Testing Plan

1. Create test branch: `fix/release-pipeline`
2. Push changes and monitor CI
3. Trigger manual `workflow_dispatch` with `dry_run: true`
4. Verify all 6 targets build successfully
5. Create test tag (e.g., `v0.21.2-rc.1`) to verify full pipeline

## Rollback Plan

If issues occur:
1. Revert to previous release.yml via `git revert`
2. Manual release using local builds
3. Document issues for next iteration

## Success Criteria

- [ ] All 6 build targets complete successfully
- [ ] Docker image pushed to ghcr.io/supernovae-st/nika
- [ ] GitHub release has 12+ assets (6 targets × 2 files each)
- [ ] Homebrew formula updated automatically
- [ ] SLSA provenance generated for Docker image

## References

- [spn-cli release.yml](../../supernovae-cli/.github/workflows/release.yml) - Working reference
- [GitHub Actions versions](https://github.com/actions) - Latest action versions
- [cross-rs](https://github.com/cross-rs/cross) - Cross-compilation tool
