# Nika v0.28 CI/CD Quick Start Guide

**Purpose:** Get the new CI/CD pipeline running in 15 minutes.

---

## Prerequisites

- [x] Rust 1.86+ installed
- [x] cargo-edit: `cargo install cargo-edit`
- [x] cargo-nextest: `cargo install cargo-nextest`
- [x] jq: `brew install jq` (macOS) or `sudo apt-get install jq` (Ubuntu)
- [x] GitHub repository with admin access
- [x] crates.io account and token

---

## Step 1: Copy Workflows (2 min)

```bash
cd /path/to/nika

# Copy CI workflow
cp docs/CI_WORKFLOW_COMPLETE.yml .github/workflows/ci.yml

# Copy Release workflow
cp docs/RELEASE_WORKFLOW_COMPLETE.yml .github/workflows/release.yml

# Verify files
ls -la .github/workflows/
```

**Expected output:**
```
ci.yml
release.yml
comprehensive-tests.yml  # Keep existing
...
```

---

## Step 2: Copy Scripts (2 min)

```bash
# Create scripts directory if needed
mkdir -p scripts

# Extract scripts from MIGRATION_SCRIPTS.md
# (Use your editor to copy each script block into separate files)

# Make scripts executable
chmod +x scripts/*.sh

# Verify
ls -la scripts/
```

**Expected scripts:**
```
verify-versions.sh
bump-version.sh
pre-publish-check.sh
publish-crates.sh
rollback-publish.sh
test-ci-locally.sh
```

---

## Step 3: Configure GitHub Secrets (3 min)

Go to: `https://github.com/YOUR_ORG/nika/settings/secrets/actions`

Add these secrets:

| Secret Name | Value | Where to Get |
|-------------|-------|--------------|
| `CRATES_IO_TOKEN` | Your token | https://crates.io/settings/tokens |
| `CODECOV_TOKEN` | Your token | https://codecov.io |
| `HOMEBREW_TAP_TOKEN` | GitHub PAT | https://github.com/settings/tokens |

**GITHUB_TOKEN** is provided automatically.

---

## Step 4: Test Locally (5 min)

```bash
# Run local CI simulation
./scripts/test-ci-locally.sh
```

**Expected output:**
```
=== Local CI Simulation ===

Phase 1/7: Format check...
✅ Format check passed

Phase 2/7: Clippy check...
  Checking nika-core...
  ✅ nika-core clippy passed
  Checking nika-runtime...
  ✅ nika-runtime clippy passed
  Checking nika-tui...
  ✅ nika-tui clippy passed

Phase 3/7: nika-core tests...
✅ nika-core tests passed

Phase 4/7: nika-runtime tests...
✅ nika-runtime tests passed

Phase 5/7: nika-tui tests...
✅ nika-tui tests passed

Phase 6/7: Documentation build...
✅ Documentation build passed

Phase 7/7: Security checks...
✅ cargo-deny passed

========================================
✅ All local CI checks passed

Ready to push to GitHub!
```

---

## Step 5: Push and Monitor (3 min)

```bash
# Create feature branch
git checkout -b feature/v0.28-workspace-ci

# Commit workflows
git add .github/workflows/ci.yml .github/workflows/release.yml
git add scripts/
git commit -m "feat: add workspace CI/CD pipeline"

# Push
git push -u origin feature/v0.28-workspace-ci
```

**Monitor CI:**
1. Go to: `https://github.com/YOUR_ORG/nika/actions`
2. Click on the running workflow
3. Watch job progress

**Expected:**
- Format check: ✅ (30s)
- Clippy check: ✅ (2 min)
- test-core: ✅ (10 min)
- test-runtime: ✅ (15 min)
- test-tui: ✅ (15 min)
- integration-tests: ✅ (15 min)
- coverage/security/docs/msrv: ✅ (20 min)
- build-verification: ✅ (10 min)
- summary: ✅ (1 min)

**Total: ~35 minutes**

---

## Step 6: Create PR and Merge

```bash
# Create PR via GitHub CLI
gh pr create \
  --title "feat: workspace CI/CD pipeline for v0.28" \
  --body "Implements 3-crate workspace CI/CD architecture with feature matrix testing and sequential crate publishing."

# Wait for CI to pass
gh pr checks

# Merge when ready
gh pr merge --squash
```

---

## Step 7: Test Release (Optional)

**Dry-run release** (won't publish):

```bash
# Trigger release workflow manually
gh workflow run release.yml \
  -f tag=v0.28.0-test \
  -f dry_run=true

# Monitor
gh run watch
```

---

## Common Issues

### Issue: "command not found: cargo-nextest"

**Fix:**
```bash
cargo install cargo-nextest
```

### Issue: "jq: command not found"

**Fix:**
```bash
# macOS
brew install jq

# Ubuntu
sudo apt-get install jq
```

### Issue: CI fails on "version mismatch"

**Fix:**
```bash
# Synchronize versions
cargo set-version --workspace 0.28.0
./scripts/verify-versions.sh
```

### Issue: Coverage below 70%

**Fix:** Add tests or adjust threshold in `.github/workflows/ci.yml`:
```yaml
env:
  COVERAGE_THRESHOLD: 65  # Lower threshold
```

---

## Verification Checklist

After Step 5 (CI running), verify:

- [ ] Format check passes
- [ ] All clippy checks pass (3 crates)
- [ ] All tests pass (core, runtime, tui)
- [ ] Integration tests pass
- [ ] Coverage >= 70%
- [ ] Security audit passes
- [ ] Documentation builds
- [ ] MSRV check passes
- [ ] Build verification completes
- [ ] PR comment shows summary

---

## Next Steps

Once CI is stable:

1. **Merge to main** — Get workspace CI running on main branch
2. **Create release** — Tag v0.28.0 and test release workflow
3. **Publish crates** — Run `./scripts/publish-crates.sh`
4. **Monitor** — Set up GitHub Actions notifications
5. **Optimize** — Tune cache keys, matrix size, timeouts

---

## Help

**Stuck?** Check these resources:

1. **Full Design:** [CI_CD_WORKSPACE_DESIGN.md](./CI_CD_WORKSPACE_DESIGN.md)
2. **Architecture:** [CI_CD_ARCHITECTURE.txt](./CI_CD_ARCHITECTURE.txt)
3. **Summary:** [CI_CD_SUMMARY.md](./CI_CD_SUMMARY.md)
4. **Scripts:** [MIGRATION_SCRIPTS.md](./MIGRATION_SCRIPTS.md)

**Still stuck?** Contact: thibaut@supernovae.studio

---

## Success Criteria

You're done when:

- [x] CI workflow runs on every PR
- [x] All 11 jobs pass consistently
- [x] Coverage maintains 70%+
- [x] PR comments show results
- [x] Build artifacts generated
- [x] Team understands new workflow

**Estimated time to production:** 4 weeks (including testing and rollout)

---

## Quick Commands Reference

```bash
# Version management
./scripts/verify-versions.sh              # Check sync
./scripts/bump-version.sh patch           # Bump version

# Testing
./scripts/test-ci-locally.sh              # Local CI
cargo nextest run --workspace             # All tests

# Pre-publish validation
./scripts/pre-publish-check.sh nika-core  # Validate crate

# Publishing (production)
export CARGO_REGISTRY_TOKEN=<token>
./scripts/publish-crates.sh               # Publish all

# Rollback
./scripts/rollback-publish.sh nika-runtime 0.28.0  # Yank

# GitHub
gh pr create                               # Create PR
gh pr checks                               # Watch CI
gh workflow run release.yml               # Manual release
```

---

**Status:** ✅ Ready to implement

**Time to complete:** 15 minutes (following this guide)

**Next:** Follow Step 1 above to begin
