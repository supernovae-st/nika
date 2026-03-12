# Nika v0.28 CI/CD Pipeline - Complete Design Summary

**Version:** v0.28.0
**Date:** 2026-03-12
**Status:** ✅ Design Complete

---

## Quick Reference

| Document | Purpose |
|----------|---------|
| [CI_CD_WORKSPACE_DESIGN.md](./CI_CD_WORKSPACE_DESIGN.md) | Complete architecture design |
| [CI_WORKFLOW_COMPLETE.yml](./CI_WORKFLOW_COMPLETE.yml) | Main CI workflow (copy to `.github/workflows/`) |
| [RELEASE_WORKFLOW_COMPLETE.yml](./RELEASE_WORKFLOW_COMPLETE.yml) | Release workflow (copy to `.github/workflows/`) |
| [MIGRATION_SCRIPTS.md](./MIGRATION_SCRIPTS.md) | Migration and automation scripts |

---

## Overview

The Nika v0.28 CI/CD pipeline supports a **3-crate workspace architecture**:

```
nika-tui (bin)
    ↓
nika-runtime (lib)
    ↓
nika-core (lib)
```

---

## Key Statistics

### CI Pipeline

| Metric | Value |
|--------|-------|
| **Total Jobs** | 11 |
| **Test Matrix** | 60 jobs (optimized from 108) |
| **Platforms** | Ubuntu, macOS, Windows |
| **Rust Versions** | stable, beta (beta=Ubuntu only) |
| **Total Time** | ~35 minutes |
| **Parallelization** | Maximum (dependency-ordered) |

### Release Pipeline

| Metric | Value |
|--------|-------|
| **Build Targets** | 6 (macOS ARM64/x64, Linux ARM64/x64, musl x64/ARM64) |
| **Crate Publishing** | Sequential with 60s propagation delays |
| **Total Time** | ~60 minutes |
| **Automation** | Full (version sync, changelog, Docker, Homebrew) |

---

## CI Pipeline Structure

### Job Graph

```
format-check ─┐
clippy-check ─┤
              ├─→ test-core ─→ test-runtime ─→ test-tui ─┐
              │                                             │
              └─────────────────────────────────────────────┤
                                                            │
┌───────────────────────────────────────────────────────────┤
│                                                           │
├─→ integration-tests ─┬─→ coverage                        │
│                      ├─→ security                        │
│                      ├─→ docs                            │
│                      └─→ msrv                            │
│                                                           │
└─→ build-verification ────────────────────────────────────→ summary
```

### Phases

1. **Phase 1: Quick Checks** (2 min)
   - Format check
   - Clippy per crate (3 jobs)

2. **Phase 2: Core Tests** (10 min)
   - nika-core on Ubuntu/macOS
   - stable + beta (Ubuntu only)

3. **Phase 3: Runtime Tests** (15 min)
   - 6 feature combinations
   - Ubuntu/macOS matrix

4. **Phase 4: TUI Tests** (15 min)
   - 5 feature combinations
   - Ubuntu/macOS matrix

5. **Phase 5: Integration** (15 min)
   - Workspace tests
   - Example validation

6. **Phase 6: Quality Gates** (20 min parallel)
   - Coverage (70% threshold)
   - Security (cargo-deny, cargo-audit)
   - Documentation build
   - MSRV check (1.86)

7. **Phase 7: Build** (10 min)
   - Release builds
   - Binary verification

8. **Phase 8: Summary** (1 min)
   - PR comment
   - Job summary

---

## Release Pipeline Structure

### Sequential Stages

1. **Version Validation** (2 min)
   - Verify workspace version coherence
   - Check CHANGELOG entries

2. **Build Matrix** (30 min)
   - 6 targets (native + musl)
   - Cross-compilation for ARM64
   - Binary packaging with checksums

3. **Test Suite** (20 min)
   - Full workspace tests
   - Binary smoke tests

4. **Publish Crates** (10 min)
   - nika-core → wait 60s
   - nika-runtime → wait 60s
   - nika-tui → verify

5. **GitHub Release** (5 min)
   - Generate release notes
   - Upload binaries
   - Attach checksums

6. **Docker Publish** (15 min)
   - Multi-arch (amd64, arm64)
   - SBOM + provenance
   - Latest + versioned tags

7. **Homebrew Update** (5 min)
   - Update formula
   - Auto-PR to tap

---

## Feature Matrix Testing

### nika-runtime Features

```yaml
- no-default
- default
- spn-daemon
- native-inference
- native-keychain
- all
```

### nika-tui Features

```yaml
- no-default
- default
- lsp
- jobs
- docker
- all
```

**Total Combinations Tested:** 12 (6 + 6)

---

## Crate Publishing Strategy

### Sequential Order (CRITICAL)

```bash
1. nika-core     → publish → wait 60s → verify
2. nika-runtime  → publish → wait 60s → verify
3. nika-tui      → publish → wait 30s → verify
```

### Propagation Delays

- **After nika-core:** 60 seconds
- **After nika-runtime:** 60 seconds
- **After nika-tui:** 30 seconds (final verification)

### Rollback Strategy

If publishing fails at any stage:

```bash
# Yank failed crate
cargo yank --vers 0.28.0 nika-runtime

# Bump PATCH version
cargo set-version --workspace --bump patch

# Retry publishing
./scripts/publish-crates.sh
```

---

## Migration Checklist

### Week 1: Preparation

- [ ] Create workspace root `Cargo.toml`
- [ ] Create crate directories: `nika-core/`, `nika-runtime/`, `nika-tui/`
- [ ] Split code into crates
- [ ] Update imports
- [ ] Verify local builds: `cargo build --workspace`
- [ ] Run tests: `cargo nextest run --workspace`
- [ ] Fix clippy warnings

### Week 2: CI Migration

- [ ] Create `.github/workflows/ci.yml` (from `CI_WORKFLOW_COMPLETE.yml`)
- [ ] Update `.github/workflows/release.yml` (from `RELEASE_WORKFLOW_COMPLETE.yml`)
- [ ] Test CI on feature branch
- [ ] Fix any CI failures
- [ ] Verify coverage maintained

### Week 3: Release Automation

- [ ] Copy migration scripts to `scripts/`
- [ ] Make scripts executable: `chmod +x scripts/*.sh`
- [ ] Test version synchronization: `./scripts/verify-versions.sh`
- [ ] Test local CI: `./scripts/test-ci-locally.sh`
- [ ] Configure crates.io token in GitHub secrets

### Week 4: Deployment

- [ ] Merge workspace PR to main
- [ ] Tag v0.28.0 release
- [ ] Monitor CI/CD pipeline
- [ ] Publish to crates.io: `./scripts/publish-crates.sh`
- [ ] Verify GitHub release created
- [ ] Verify Docker image available
- [ ] Update documentation

---

## Scripts Reference

### Version Management

```bash
# Check version synchronization
./scripts/verify-versions.sh

# Bump version (patch/minor/major)
./scripts/bump-version.sh patch
```

### Pre-Publish

```bash
# Validate crate ready for publishing
./scripts/pre-publish-check.sh nika-core
```

### Publishing

```bash
# Publish all crates in order
export CARGO_REGISTRY_TOKEN=<token>
./scripts/publish-crates.sh
```

### Rollback

```bash
# Yank failed publish
./scripts/rollback-publish.sh nika-runtime 0.28.0
```

### Testing

```bash
# Run local CI simulation
./scripts/test-ci-locally.sh
```

---

## GitHub Actions Secrets

Required secrets for CI/CD:

| Secret | Purpose | Where to Get |
|--------|---------|--------------|
| `CRATES_IO_TOKEN` | Publishing to crates.io | https://crates.io/settings/tokens |
| `CODECOV_TOKEN` | Coverage reporting | https://codecov.io |
| `HOMEBREW_TAP_TOKEN` | Homebrew formula updates | GitHub PAT with repo access |
| `GITHUB_TOKEN` | GitHub API (auto-provided) | Automatic |

---

## Monitoring & Observability

### CI Dashboard

Monitor pipeline status at:
```
https://github.com/supernovae-st/nika/actions
```

### Key Metrics to Track

1. **Build Time:** Target <35 min for full CI
2. **Test Pass Rate:** Maintain 100%
3. **Coverage:** Keep above 70%
4. **Clippy Warnings:** Zero tolerance
5. **Security Vulnerabilities:** Zero high/critical

### Alerts

Set up GitHub Actions notifications for:
- CI failures on main branch
- Release workflow failures
- Security audit findings

---

## Optimization Opportunities

### Current Optimizations

1. **Beta testing:** Ubuntu only (reduce 48 jobs)
2. **Parallel phases:** Independent jobs run concurrently
3. **Rust cache:** Aggressive caching with Swatinem/rust-cache
4. **Nextest:** Faster test execution than `cargo test`

### Future Optimizations

1. **Sccache:** Distributed compilation cache
2. **Custom runners:** Self-hosted for faster builds
3. **Matrix pruning:** Skip redundant feature combinations
4. **Artifact reuse:** Cache between test and build phases

---

## Troubleshooting

### Common Issues

**Issue:** Version mismatch during release

```bash
# Fix: Synchronize versions
cargo set-version --workspace 0.28.1
./scripts/verify-versions.sh
```

**Issue:** Crate publish fails with "already published"

```bash
# Fix: Bump PATCH version
./scripts/bump-version.sh patch
./scripts/publish-crates.sh
```

**Issue:** CI timeout on macOS

```bash
# Fix: Increase timeout in workflow
timeout-minutes: 60  # Default is 360
```

**Issue:** Coverage below threshold

```bash
# Fix: Add tests or adjust threshold
COVERAGE_THRESHOLD: 65  # Lower if needed
```

---

## Best Practices

### Development Workflow

1. **Branch naming:** `feature/v0.28-workspace`
2. **Commit messages:** Conventional commits (`feat:`, `fix:`, `chore:`)
3. **PR size:** Keep PRs focused and reviewable
4. **Testing:** Run `./scripts/test-ci-locally.sh` before pushing

### Release Workflow

1. **Version bumping:** Use `./scripts/bump-version.sh`
2. **CHANGELOG:** Update before tagging
3. **Testing:** Full CI must pass before release
4. **Publishing:** Always use `./scripts/publish-crates.sh`
5. **Verification:** Check crates.io after publishing

---

## Success Criteria

- [ ] All 11 CI jobs pass on every PR
- [ ] Coverage maintained at 70%+
- [ ] Zero clippy warnings
- [ ] All crates build on all platforms
- [ ] Release completes in <60 minutes
- [ ] Crates publish successfully in order
- [ ] Docker image builds for both architectures
- [ ] Homebrew formula updates automatically

---

## Next Steps

1. **Immediate:**
   - Copy workflows to `.github/workflows/`
   - Copy scripts to `scripts/`
   - Test on feature branch

2. **Short-term:**
   - Complete workspace code splitting
   - Update documentation
   - Train team on new workflows

3. **Long-term:**
   - Monitor CI performance
   - Optimize based on metrics
   - Add more platforms if needed

---

## Support

For questions or issues:

1. Check this documentation
2. Review GitHub Actions logs
3. Test locally with `./scripts/test-ci-locally.sh`
4. Contact: thibaut@supernovae.studio

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-03-12 | Initial design complete |

---

**Status:** ✅ Ready for implementation

**Estimated Implementation Time:** 4 weeks

**Risk Level:** Medium (requires careful migration and testing)

**Impact:** High (improves modularity, maintainability, publishing workflow)
